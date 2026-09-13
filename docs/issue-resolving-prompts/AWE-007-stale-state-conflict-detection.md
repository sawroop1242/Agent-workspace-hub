# AWE-007 / #28 — Stale-State and Edit Conflict Detection

## Mission

Implement production-grade stale-state and edit conflict detection for the canonical AWH editing pipeline.

The implementation must make a caller's expected file state a real precondition of mutation rather than metadata that is merely stored or ignored.

The non-negotiable safety invariant is:

```text
expected_state != current_state
    → conflict
    → zero mutation
```

A stale edit must never silently overwrite newer content, reinterpret a caller's old assumptions against newer bytes, or replace the caller's expected state with the current state merely to make the operation succeed.

This issue is a P0 reliability requirement and must be implemented as a transport-independent service/domain behavior. MCP, CLI, TUI, and Control API must consume the same conflict semantics rather than implementing their own checks.

---

## 1. Required preflight — understand the repository before editing

Before changing code, inspect the current `rust` branch and establish the actual implementation boundary. Do not infer that planned APIs exist merely because they are documented.

Read at minimum:

- `docs/PROJECT_CONTEXT.md`
- `docs/PROJECT_ROADMAP.md`
- `docs/PROJECT_ROADMAP_STATUS.md`
- `docs/PROJECT_STATUS.md`
- `docs/FEATURES.md`
- `docs/CLI.md`
- `docs/architecture.md`
- `docs/mcp.md`
- `docs/issue-resolving-prompts/AWE-007-stale-state-conflict-detection.md`
- the implementation and tests for `src/services/edit.rs`
- the existing workspace/filesystem write path
- relevant path-security and atomic-write helpers
- relevant MCP/tool error/serialization boundaries

Search the repository before introducing new abstractions. In particular search for:

- `ExpectedState`
- `FileState`
- `workspace.write_file`
- `EditTransaction`
- `EditError`
- `EditStatus::Conflict`
- `atomic`
- `write_file`
- `read_file`
- existing conflict/error types
- existing filesystem state/hash helpers

Do not create a second hash implementation, path validator, atomic writer, structured error model, or filesystem-state abstraction if an existing primitive can safely be extended.

The forensic baseline for this issue is important: the canonical edit model already defines `ExpectedState`, `FileState`, and a `Conflict` lifecycle state, but there is no complete production edit executor enforcing the expected-state precondition. The existing workspace write capability can therefore overwrite newer content without an expected-state guard. The implementation must close that reliability gap rather than merely adding more fields to the existing model.

---

## 2. Dependency contract

This issue builds on the canonical editing work established by the preceding milestones:

```text
AWE-001  canonical EditTransaction
    ↓
AWE-002  safe contextual replacement
    ↓
AWE-003  line insert/delete semantics
    ↓
AWE-004  multi-operation filesystem patch
    ↓
AWE-005  unified-diff preparation/application
    ↓
AWE-006  atomic + rollback-safe mutation
    ↓
AWE-007  stale-state / conflict detection   ← THIS ISSUE
```

Use the existing transaction model and do not fork the edit architecture.

AWE-007 must preserve the invariant during the complete mutation lifecycle, including preparation, commit, verification, and rollback boundaries established by AWE-006.

Do not implement later milestones while solving this issue.

---

## 3. Canonical stale-state model

Treat the file state observed by the editor as an immutable precondition.

The canonical state vocabulary currently includes:

```text
ExpectedState
    hash: Option<String>
    context: Option<String>
    size: Option<u64>
    line_count: Option<usize>

FileState
    path: String
    hash: String
    size: u64
    line_count: usize
```

`ExpectedState` means: "the file is still sufficiently identical to the state against which this edit was planned."

`FileState` means: "this is what the filesystem actually contains at the observation boundary."

Do not weaken this into a best-effort warning.

If a caller supplied a precondition and that precondition does not hold, the operation must return a machine-readable conflict and must not mutate the target.

---

## 4. Define exact comparison semantics

Implement and document deterministic semantics for every expected-state field.

### 4.1 Hash

When `ExpectedState.hash` is present:

- read the current file bytes before mutation;
- calculate the canonical SHA-256 using the existing project helper where possible;
- compare it exactly with the expected digest;
- a mismatch is a conflict;
- never continue to a write after a hash mismatch.

Hash comparison is the strongest complete-file state check currently represented by the model.

Do not normalize, trim, re-encode, rewrite line endings, or otherwise transform bytes before calculating the hash.

### 4.2 Size

When `ExpectedState.size` is present, compare the current byte length with the expected value.

A mismatch is a conflict.

Size must be calculated from the same byte representation used by the actual edit pipeline, not from a potentially normalized string representation.

### 4.3 Line count

When `ExpectedState.line_count` is present, compare it using the canonical line-count semantics already defined by the edit model.

Do not silently invent a second line-count algorithm for conflict detection.

Any newline/EOF behavior used here must remain consistent with AWE-003 and the actual edit executor.

### 4.4 Context

`ExpectedState.context` must have explicit, operation-aware semantics. It cannot simply be stored and ignored.

For contextual replacement/patch operations, the expected context must still exist at the intended edit location before mutation.

For line operations, expected context must protect the relevant line boundary/range rather than merely proving that some unrelated copy of the same text exists elsewhere in the file.

For operations where a precise location cannot be represented by the current context field, use the strongest safe semantics supported by the canonical operation model and fail closed rather than pretending that an ambiguous context match is sufficient.

Context comparison must be deterministic and must not silently select a different occurrence when the caller's original target has become stale.

---

## 5. Operation-specific conflict protection

Apply the expected-state contract consistently across every canonical edit operation that accepts an expected state:

- `Replace`
- `Insert`
- `DeleteRange`
- `Patch`
- `ApplyDiff`

The implementation must not protect `Replace` while leaving line operations vulnerable.

### Replace / Patch

Validate the expected complete-file state and intended context before preparing the mutation.

If the caller planned against an older version of the file, reject it before any write.

Do not search the current file and silently reinterpret an old edit against a changed structure.

### Insert

Protect the line boundary against stale content. A file can change without its line count changing, so line count alone is not sufficient.

When context is available, it must bind the insertion to the expected surrounding content.

### DeleteRange

Protect both the expected file state and the intended range context. A stale line range is dangerous because another edit can shift lines while leaving a superficially valid range in place.

A changed line at or around the requested range must cause a conflict when the caller's expected state no longer holds.

### ApplyDiff

Unified-diff application must retain its exact-context semantics and must not bypass the transaction-level expected-state check. A patch that no longer applies cleanly is a conflict, not permission to guess.

---

## 6. Validation ordering — conflict must happen before mutation

The canonical pipeline must establish this ordering:

```text
request
  ↓
shape validation
  ↓
path/security validation
  ↓
read current state
  ↓
compare expected state
  ↓
context/operation validation
  ↓
prepare mutation
  ↓
commit atomically
  ↓
verify
```

For a stale-state failure:

```text
read current state
  ↓
comparison fails
  ↓
Conflict
  ↓
NO mutation
```

No temporary replacement file, rename, truncate, append, partial write, or other mutation may occur before a failed expected-state check.

For a multi-file transaction, all affected files must have their expected state validated before the commit phase. A conflict in file N must not leave files 1..N-1 modified.

Reuse AWE-004 preparation/commit separation and AWE-006 atomic/rollback semantics rather than inventing a second transaction algorithm.

---

## 7. Deterministic machine-readable conflict result

Conflict is a first-class domain result, not merely a string error message.

The result must make it possible for an MCP/CLI caller to determine:

- transaction/edit ID;
- affected path;
- operation index/type where applicable;
- expected state supplied by the caller;
- actual state observed from the filesystem;
- which precondition(s) failed;
- whether any mutation occurred;
- whether rollback was required;
- safe recovery guidance.

A useful conceptual payload is:

```text
Conflict {
    edit_id
    path
    operation_index
    operation
    expected
    actual
    mismatches
    mutation_applied: false
    recovery: "reread → recompute → resubmit"
}
```

Adapt the exact Rust type to the repository's existing error architecture. Do not create an incompatible parallel result system merely to match this example.

Expected and actual state must be structured data, not only interpolated into a prose message.

Do not expose secrets or unrelated file contents through the conflict payload.

---

## 8. Transaction lifecycle

Use the existing lifecycle vocabulary where applicable:

```text
Requested
  → Authorized
  → Located
  → Validated
  → Snapshotted
  → Applied
  → Verified
  → Committed
```

A stale expected-state precondition must transition into the canonical conflict outcome rather than `ApplyFailed` or an opaque generic I/O error.

The important distinction is:

```text
invalid request      → validation failure
missing/invalid path → path/security failure
stale expected state → conflict
filesystem failure   → apply failure
post-write mismatch  → verification failure
```

Do not misclassify a normal concurrent edit as an exceptional internal failure.

The issue explicitly treats a race between read and edit as a normal concurrency condition.

---

## 9. Read → modify → resubmit recovery contract

Conflict recovery must be safe and explicit:

```text
conflict
   ↓
reread current file/state
   ↓
recompute intended edit from current content
   ↓
construct a new expected state
   ↓
resubmit
```

Never implement this automatically by overwriting the caller's `ExpectedState` with the current state.

The system may provide recovery metadata or guidance, but it must not silently convert a stale edit into a fresh edit.

If the agent wants to apply the edit to the new state, that must be a new caller decision and a new expected-state precondition.

---

## 10. TOCTOU and concurrent mutation handling

Do not claim that a separate `stat/read → compare → write` sequence eliminates every race.

A second process can modify a file after the expected-state check but before the actual commit.

The implementation must therefore:

1. explicitly identify the read/check/commit race window;
2. reuse the strongest existing atomic filesystem primitives;
3. coordinate mutation where the repository already provides a lock or transaction boundary;
4. avoid claiming stronger concurrency guarantees than the implementation and tests prove;
5. preserve AWE-006 rollback safety if a mutation is detected as stale during a later boundary.

If a race-resistant OS primitive or locking mechanism is not currently available, document the remaining limitation and keep the behavior fail-safe.

Do not introduce a fake "thread-safe" guarantee merely because the Rust code uses `&mut` references or an in-process mutex.

Cross-process tests are especially important because an in-process lock does not protect against another process modifying the workspace.

---

## 11. Conflict-aware rollback integration

AWE-007 must integrate with AWE-006 without taking over AWE-006's complete implementation scope.

Rollback must never blindly restore an old snapshot over unrelated newer changes.

If rollback itself uses an expected-state/precondition, verify the state that rollback expects before restoring it.

If the rollback target has changed unexpectedly, surface a rollback conflict rather than destroying the newer content.

The implementation must distinguish:

```text
original edit conflict
    vs.
rollback conflict
```

Do not silently convert either into success.

Do not implement AWE-010's user-facing historical rollback feature here.

---

## 12. Security and path identity

Expected-state validation must operate on the same canonical, security-validated target that the mutation will eventually modify.

Preserve all existing filesystem security invariants:

- relative workspace paths only where required by the current API;
- no `../` traversal;
- no absolute-path escape;
- no symlink escape outside the allowed workspace;
- reuse existing canonical path/security helpers;
- fail closed when the target cannot be safely resolved.

Do not compare the state of one path and then mutate a different path because of symlink or path-resolution changes.

The conflict mechanism must not become a bypass around existing policy or workspace containment checks.

---

## 13. Encoding, Unicode, and line-ending correctness

Conflict detection must work for real source files, not only ASCII/LF fixtures.

Tests must cover:

- UTF-8;
- Devanagari/Hindi text;
- emoji;
- LF files;
- CRLF files;
- files with and without a final newline;
- empty files;
- single-line files;
- multi-line files;
- edits that change line count;
- edits that preserve line count but change bytes.

Hash comparison must remain byte-exact.

Context comparison must use the same text representation as the operation that consumes it, without accidentally normalizing away meaningful differences.

---

## 14. Testing requirements

Add focused tests for both success and failure paths.

### Unit tests

At minimum test:

- matching hash succeeds;
- hash mismatch produces conflict;
- matching size succeeds;
- size mismatch produces conflict;
- matching line count succeeds;
- line-count mismatch produces conflict;
- context match succeeds;
- context mismatch produces conflict;
- multiple supplied preconditions are all evaluated deterministically;
- expected-state comparison does not mutate the filesystem;
- conflict payload contains expected and actual state;
- conflict result identifies the failing operation/path;
- no silent expected-state replacement occurs.

### Operation tests

Test stale-state protection for:

- replace;
- insert;
- delete-range;
- patch;
- unified diff where that operation is active in the current implementation.

### External modification tests

For each relevant operation:

```text
1. read/prepare expected state
2. modify the file externally
3. attempt the original edit
4. assert Conflict
5. assert exact bytes remain equal to the externally modified content
```

The test must prove that stale content was not written over the external modification.

### Multi-file tests

Prepare a transaction affecting multiple files.

Change one target externally after the original state was captured.

Assert:

- the transaction reports the conflict;
- no file receives a partial commit;
- unaffected files remain byte-for-byte unchanged;
- the externally modified file remains byte-for-byte unchanged.

### Concurrency tests

Where practical, use at least two tasks/processes to reproduce:

```text
reader/editor A captures expected state
        ↓
mutator B changes file
        ↓
editor A attempts mutation
        ↓
Conflict
```

Do not rely only on two threads sharing an in-process mutex; include a real filesystem/process boundary where the test architecture permits it.

### Rollback tests

Verify that a conflict discovered during a later transaction boundary does not cause rollback to overwrite a newer external modification.

### Regression tests

All existing AWE-001 through AWE-006 tests must continue to pass.

---

## 15. Property-oriented / adversarial coverage

Where practical, add property-oriented tests for state comparison and mutation safety.

Useful invariants include:

```text
If expected state does not match current state,
then final bytes == bytes observed before attempted mutation.
```

```text
Changing any byte covered by the complete-file hash
must invalidate a matching expected hash.
```

```text
A stale line operation must never silently target a different line
because another edit shifted the file.
```

```text
Conflict detection is deterministic for identical expected/current states.
```

Test adversarial changes such as:

- changing one character;
- changing only whitespace;
- changing only line endings;
- inserting a line before the intended range;
- deleting a line before the intended range;
- changing another occurrence of the same contextual text;
- changing file size while preserving line count;
- preserving size while changing content.

---

## 16. MCP / CLI / API compatibility

The editing implementation must remain transport-independent.

MCP, CLI, TUI, and Control API layers should consume the same underlying conflict result.

For MCP-facing behavior, ensure conflict data can be serialized into the project's established machine-readable error/result format.

For CLI behavior, ensure the user can distinguish a stale-state conflict from validation, permission, filesystem, and internal failures.

Do not build a separate MCP-only stale-state detector.

Do not add a new CLI command unless the current milestone genuinely requires one.

The core service remains authoritative.

---

## 17. Observability and information hygiene

Where the existing architecture supports structured logging/audit/provenance, record the conflict event with safe metadata such as:

- edit ID;
- operation/path identity where permitted;
- conflict category;
- expected/actual hashes or safe state metadata;
- transaction status;
- duration if already supported.

Never log:

- secrets;
- authentication tokens;
- unnecessary full file contents;
- sensitive contextual text unless existing policy explicitly permits it.

Do not add a new audit system for this issue. Integrate with existing primitives only.

---

## 18. Performance and resource behavior

Do not read a file multiple times unnecessarily when one safely reusable state observation can serve validation and preparation.

Avoid storing entire duplicate copies of large files solely for conflict reporting when the existing transaction architecture already provides safer alternatives.

Do not remove hash/context checks to improve performance without an explicit architectural decision.

If a resource limit is required for extremely large files, fail explicitly and safely rather than silently skipping stale-state validation.

Correctness and safety take priority over micro-optimization for this P0 path.

---

## 19. Backward compatibility

Preserve the serialized shape of existing edit types unless a change is genuinely required for the conflict contract.

Existing callers that omit `ExpectedState` must retain their documented behavior; however, when an expected-state precondition is supplied, it must be enforced.

Do not silently reinterpret an omitted expected state as an automatically generated current-state precondition unless the existing API contract explicitly requires that behavior.

Do not break existing `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, or `EditStatus` serialization without a clear compatibility strategy.

---

## 20. Explicit non-goals — do not scope-creep

Do **not** use AWE-007 to implement unrelated future milestones, including:

- new MCP transports;
- new agent profiles or agent identity architecture;
- worktree isolation;
- snapshot storage redesign;
- historical edit rollback UI/API;
- persistent audit redesign;
- TUI redesign;
- remote AWH;
- generic distributed locking infrastructure;
- a generic workflow engine;
- model routing;
- external connector integrations;
- broad filesystem API redesign;
- unrelated security refactors;
- AWE-008 post-edit verification as a separate milestone;
- AWE-009 MCP agent-grade editing surface;
- AWE-010 explicit edit-level rollback;
- AWE-011 capability-policy edit paths;
- AWE-012 snapshots/provenance;
- AWE-013 persistent audit;
- AWE-014 CLI editing surface;
- AWE-015 full editing test-suite milestone;
- AWE-016 real MCP-client validation;
- AWE-017 end-to-end acceptance workflow;
- AWE-018 final editing contract.

Implement only the conflict-detection capability and the minimum integration required to make that capability real and testable.

---

## 21. Code quality requirements

Follow the repository's existing Rust architecture and conventions.

Prefer:

- small, composable functions;
- typed errors with `thiserror` where callers need to match conflict categories;
- `anyhow::Context` only at application/runtime boundaries where appropriate;
- existing filesystem/security helpers;
- explicit state transitions;
- clear Rustdoc on public conflict types/functions;
- deterministic tests;
- no unnecessary dependencies.

Do not use `unwrap`, `expect`, or panic paths in production conflict handling unless an existing repository convention makes the invariant provably unreachable and the surrounding code already follows that pattern.

Avoid duplicating validation/error formatting.

---

## 22. Verification commands

After implementation, run the relevant repository gates, at minimum:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also run focused tests for the editing/conflict subsystem.

Inspect the final diff carefully:

```bash
git status --short
git diff --check
git diff
```

Confirm that the implementation:

- detects stale expected state before mutation;
- returns structured conflict information;
- preserves exact bytes on conflict;
- handles external modification;
- protects line operations;
- integrates with multi-file transactions;
- remains compatible with atomic/rollback behavior;
- does not weaken existing path/security controls.

---

## 23. Definition of Done

AWE-007 is complete only when all of the following are true:

- [ ] `ExpectedState.hash` is enforced before mutation.
- [ ] `ExpectedState.context` has explicit and operation-appropriate semantics.
- [ ] `ExpectedState.size` is enforced when supplied.
- [ ] `ExpectedState.line_count` is enforced when supplied.
- [ ] Line operations are protected against stale line context.
- [ ] A stale expected state produces a first-class conflict.
- [ ] Conflict results include expected and actual state in machine-readable form.
- [ ] Conflict results identify the relevant transaction/operation/path.
- [ ] A stale-state failure performs zero mutation.
- [ ] External modification tests prove newer bytes are never overwritten.
- [ ] Multi-file transactions validate all expected states before commit.
- [ ] Concurrent mutation scenarios are tested where practical.
- [ ] Rollback cannot blindly overwrite a newer external modification.
- [ ] MCP/CLI-facing layers can distinguish conflict from generic failure.
- [ ] Existing security/path validation remains intact.
- [ ] Unicode, CRLF/LF, EOF, empty-file, and line-count edge cases are covered.
- [ ] Existing AWE-001 through AWE-006 behavior remains green.
- [ ] Formatting, check, tests, and Clippy pass.
- [ ] No unrelated files or future milestones were implemented.
- [ ] Final diff confirms the requested scope only.

---

## 24. Final implementation report

When the implementation is complete, report:

1. files changed;
2. exact conflict-detection behavior implemented;
3. expected-state comparison semantics;
4. operation types protected;
5. structured conflict result/error design;
6. concurrency/TOCTOU guarantees and remaining limitations;
7. tests added;
8. verification commands and results;
9. any known follow-up limitations;
10. confirmation that no stale edit can silently overwrite newer content.

Do not claim stronger crash consistency, cross-process synchronization, or race freedom than the tests and implementation establish.

---

## HARD STOP — AWE-007 ONLY

Implement **only AWE-007 / GitHub issue #28: stale-state and edit conflict detection**.

Do not advance to AWE-008 or modify its prompt. Do not implement later editing milestones. Do not redesign unrelated subsystems.

The final implementation must preserve the central invariant:

```text
expected_state != current_state
    → machine-readable Conflict
    → zero mutation
```

AWH must never resolve a stale edit by silently accepting the newer state on behalf of the caller.

After AWE-007 is implemented, tested, diff-audited, and committed, **STOP** and wait for the next explicit task.