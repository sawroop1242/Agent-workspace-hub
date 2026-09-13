# AWE-008 / #29 — Post-Edit Verification

## Mission

Implement production-grade, transport-independent post-edit verification for the canonical AWH editing pipeline so that an agent-grade mutation is never reported successful merely because a write or atomic replacement completed.

The implementation must verify the **actual resulting filesystem state**, capture the resulting `FileState`, distinguish semantic verification failure from apply failure and stale-state conflict, and integrate with AWE-006 rollback when verification fails after mutation.

This prompt is the complete implementation contract for AWE-008. Implement only AWE-008 / issue #29. Do not implement AWE-009 or any later editing milestone. Do not broaden the task into MCP, CLI, TUI, Control API, snapshots, audit, agent profiles, worktrees, or project/build orchestration except where existing shared interfaces must remain compatible.

---

## 1. Authoritative repository context

Repository: `sawroop1242/Agent-workspace-hub`

Target branch: `rust`

Target issue: `#29 — AWE-008: Add post-edit verification`

Target architecture: AWH is an agent-agnostic, local-first workspace runtime. Editing is a shared application-service concern; MCP, CLI, TUI, and Control API are interfaces over common services and must not acquire independent edit semantics.

The roadmap defines the agent-grade editing progression as:

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

The current canonical model in `src/services/edit.rs` provides `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, `EditStatus`, and stable error vocabulary, but mutation/verification behavior is intentionally implemented in later service work.

The existing `FileState` captures:

- canonical path
- SHA-256 content hash
- byte size
- line count

`EditTransaction` already has `before`, `after`, and lifecycle states including `Applied`, `Verified`, `Committed`, `ApplyFailed`, `VerificationFailed`, and `RolledBack`.

Treat the repository's current source as authoritative. Do not assume a service exists simply because the architecture documents mention it.

---

## 2. Issue requirements — non-negotiable baseline

The issue requires:

- verification of the target's expected existence/deletion state
- readability verification where text verification applies
- confirmation that the intended operation actually changed the intended state
- capture of actual resulting hash, byte size, and line count
- structured verification failure
- rollback integration when verification fails
- verification by default for agent-grade mutations
- after-state computed from the actual filesystem, not only from an in-memory candidate
- verification failure must never be reported as success
- tested rollback integration
- optional verification hooks with bounded and explicit contracts
- practical post-write corruption/failure-injection tests

The editor must **not** become a compiler, test runner, formatter, linter orchestrator, or generic build system.

---

## 3. Required preflight: establish repository reality first

Before changing code:

1. Read and understand:
   - `docs/PROJECT_CONTEXT.md`
   - `docs/PROJECT_ROADMAP.md`
   - `docs/PROJECT_ROADMAP_STATUS.md`
   - `docs/PROJECT_STATUS.md`
   - `docs/FEATURES.md`
   - `docs/CLI.md`
   - `docs/architecture.md`
   - `docs/mcp.md`
2. Inspect the current AWE-001 through AWE-007 implementation state.
3. Inspect `src/services/edit.rs` and all current filesystem/atomic-write helpers used by editing.
4. Locate the AWE-004 transaction executor and AWE-006 rollback/recovery implementation if they exist on the current branch.
5. Identify the actual mutation entry points rather than creating a parallel editor.
6. Inspect existing error/result/status conventions and reuse them where possible.
7. Inspect current tests before adding new ones.
8. Run the relevant baseline tests before modification where practical.

Do not infer that a feature is implemented because it is named in a roadmap or documentation file. Record the actual implementation boundary mentally and build on it.

---

## 4. Core invariant

The central AWE-008 invariant is:

```text
successful mutation
    =
actual filesystem state satisfies the operation's postcondition
AND
actual resulting state has been captured
AND
verification completed successfully
```

Therefore:

```text
validate
→ prepare
→ stale-state/conflict check
→ snapshot boundary
→ apply
→ read actual filesystem
→ verify postcondition
→ capture actual after-state
→ commit
```

Failure path:

```text
apply failure
    → ApplyFailed

verification failure
    → VerificationFailed
    → rollback when transaction recovery is available
    → RolledBack if restoration succeeds

rollback failure
    → never claim success
    → preserve/report recovery failure with original verification failure context
```

Never use the in-memory candidate alone as proof of the resulting filesystem state.

---

## 5. Verification must inspect the real filesystem

After every successful mutation step that is eligible for agent-grade verification:

1. Re-open/stat the actual target from the filesystem.
2. Do not reuse stale pre-write buffers as the authoritative after-state.
3. Recompute:
   - existence
   - readability where applicable
   - SHA-256 hash
   - byte size
   - line count for text content
4. Store the resulting state in `EditTransaction.after` using the canonical `FileState` representation.
5. Only transition to `Verified` after all required postconditions pass.
6. Only transition to `Committed` after verification and the transaction's commit rules succeed.

If the file disappeared unexpectedly, appears when deletion was expected, cannot be read, or has unexpected bytes, verification must fail.

---

## 6. Operation-specific postconditions

Verification must be semantic, not merely "the file exists".

### 6.1 Replace

For `Replace`:

- confirm the target exists when it should remain present
- read actual resulting text
- verify the intended replacement semantics against the actual result
- if a specific occurrence count was requested, verify the resulting occurrence behavior precisely
- ensure the operation did not silently become a no-op when a mutation was required
- capture actual after-state

Do not rely solely on comparing the candidate string generated before the write.

### 6.2 Insert

For `Insert`:

- confirm target existence
- read the actual resulting text
- verify inserted content is present at the intended line boundary
- verify surrounding content remains consistent with the operation's semantics
- distinguish a legitimate inserted empty string, if allowed by existing contracts, from a failed mutation rather than inventing new semantics
- capture actual after-state

### 6.3 DeleteRange

For `DeleteRange`:

- confirm target existence
- read the actual resulting text
- verify the requested inclusive line range is actually absent according to the canonical line semantics
- verify unaffected content remains in the correct order
- handle deletion of all content and final-newline behavior according to AWE-003 semantics
- capture actual after-state

### 6.4 Patch

For `Patch`:

- confirm the target exists
- verify the intended old content is no longer present according to the operation's established semantics
- verify the new content is present where expected
- do not treat a write as verified merely because the file can be read
- capture actual after-state

### 6.5 ApplyDiff

If AWE-005 unified-diff execution is already present on the branch, verify the resulting filesystem against the actual diff semantics and all affected paths.

If AWE-005 is not implemented, do not invent a second diff engine merely for AWE-008. Keep the verification abstraction compatible with the existing `ApplyDiff` model and document any intentionally unreachable verification path.

---

## 7. No-op detection and intended-state verification

A write succeeding at the filesystem level is not sufficient.

The verification layer must distinguish:

- successful intended mutation
- valid operation whose resulting state is intentionally unchanged, if the established operation contract permits that case
- failed/no-op mutation where a change was required
- externally altered result
- corrupted or unreadable result

Do not invent generic "content must always change" behavior without checking the operation's semantics. For example, replacing text with identical text may be a semantic no-op and must be handled deterministically rather than incorrectly classified through an ad hoc rule.

The important invariant is that the **requested postcondition**, not merely byte inequality, determines success.

---

## 8. Before/after FileState contract

The transaction must have trustworthy state boundaries:

### Before state

`before` represents the state observed and accepted before mutation.

### After state

`after` must represent the state observed from the filesystem after mutation/verification.

Never populate `after` solely by transforming `before` in memory.

For each affected file, capture at minimum:

```text
path
hash
size
line_count
```

If a target is expected to be deleted, the result contract must represent deletion explicitly rather than fabricating a `FileState` for nonexistent bytes. Reuse existing transaction/result vocabulary where available; if a new representation is genuinely required, keep it narrowly scoped to verification and preserve serialization compatibility.

---

## 9. Verification status/lifecycle

Respect the canonical lifecycle already defined in `EditStatus`:

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

Failure states include:

```text
Rejected
Conflict
ValidationFailed
ApplyFailed
VerificationFailed
RolledBack
```

Rules:

- `Applied` does not mean successful completion.
- `Verified` means actual postconditions have been checked against the filesystem.
- `Committed` must never be reached before required verification.
- `VerificationFailed` must be distinguishable from `Conflict` and `ApplyFailed`.
- If AWE-006 recovery executes after verification failure, expose the resulting rollback state accurately.
- Do not hide rollback failure behind the original verification error.

---

## 10. Structured verification errors

Verification failures must be machine-actionable and deterministic.

The error contract should make it possible to identify at least:

- transaction/edit ID
- operation index where applicable
- target path where applicable
- expected postcondition category
- actual observed state when safe/useful
- expected existence state
- actual existence state
- readability failure where applicable
- expected/actual hash when relevant
- expected/actual size when relevant
- expected/actual line count when relevant
- semantic postcondition mismatch
- whether rollback was attempted
- whether rollback succeeded or failed

Avoid returning only a free-form string such as `verification failed`.

Do not leak sensitive file contents into errors. Prefer bounded metadata and structured categories over dumping full file contents.

---

## 11. Failure-injection design

Build tests that prove the verification layer can detect a mismatch between the intended result and the actual filesystem.

Where practical, introduce test seams rather than production-only hacks.

Examples:

1. Mutation writes expected candidate; test hook alters file before verification.
2. Mutation writes expected candidate; target is deleted before verification.
3. Mutation writes expected candidate; target becomes unreadable before verification where the platform permits testing this reliably.
4. Mutation writes wrong bytes; verifier must detect hash/postcondition mismatch.
5. Multi-file transaction mutates one or more files, then verification fails; AWE-006 rollback must restore the original bytes.
6. Verification itself encounters an I/O error; transaction must not be reported successful.

Failure injection must be deterministic and isolated to tests. Do not weaken production verification to make tests easier.

---

## 12. AWE-006 rollback integration

AWE-008 depends on AWE-006.

When verification fails after mutation:

```text
verify
  ↓ failure
rollback prepared/committed mutations according to AWE-006
  ↓
verify restoration where AWE-006 requires it
  ↓
report VerificationFailed / RolledBack accurately
```

Requirements:

- rollback must use the transaction's established recovery boundary
- rollback must not blindly overwrite an externally modified file if AWE-006 conflict rules prohibit that action
- original bytes must be restored exactly when rollback is valid
- temporary artifacts must be cleaned according to existing recovery semantics
- verification failure must remain visible even when rollback succeeds
- rollback failure must be reported distinctly

Do not implement a second rollback mechanism inside AWE-008.

---

## 13. Multi-file transactions

For transactions affecting multiple files:

1. Apply according to AWE-004/AWE-006 transaction semantics.
2. Verify every affected target against its operation-specific postcondition.
3. Capture actual after-state for every successfully observed resulting target.
4. If any required verification fails before commit, do not report the transaction successful.
5. Invoke existing rollback/recovery behavior when applicable.
6. Verify or otherwise establish recovery according to AWE-006.

The transaction must never appear `Committed` merely because some files verified successfully.

Avoid partial `after` state that falsely suggests the entire transaction was verified. If partial observations must be retained for diagnostics, keep them explicitly associated with a failed transaction.

---

## 14. Stale-state and TOCTOU interaction

AWE-007 owns stale-state conflict detection. AWE-008 must consume that contract rather than duplicate it.

Verification must still account for changes occurring between mutation and verification.

The implementation must:

- use canonical filesystem/path handling already established by earlier milestones
- not assume that the state read immediately before mutation remains authoritative afterward
- distinguish stale-state conflict from post-edit verification failure
- detect unexpected external changes where the verification contract can prove them
- avoid claiming race-free semantics unless the underlying filesystem coordination actually provides them

Do not overclaim that read → write → read alone eliminates TOCTOU races. Document the actual guarantee provided by the current implementation.

---

## 15. Text verification correctness

Where text verification applies, preserve the semantics established by earlier edit milestones.

Tests must cover at least:

- empty files
- one-line files
- multiple lines
- final newline present
- final newline absent
- LF
- CRLF
- Unicode
- Devanagari text
- emoji/multi-byte UTF-8
- replacement content containing newline characters
- insertion at beginning/middle/end
- deletion of first/middle/last/all lines

Do not accidentally compute byte length as character count or line count as byte count.

`FileState.size` is bytes. Hash is over actual file bytes. Line-count behavior must remain consistent with the canonical existing implementation unless the milestone explicitly requires a correction backed by tests.

---

## 16. Binary and non-text files

Do not turn the edit engine into a generic binary semantic verifier.

For text operations:

- read as text using the existing encoding contract
- verify text semantics
- calculate hashes/sizes from actual bytes

For unsupported binary semantic verification:

- fail explicitly or use byte/state verification appropriate to the existing operation contract
- do not silently decode arbitrary binary data as UTF-8
- do not introduce a new binary patching subsystem under AWE-008

---

## 17. Optional verification hooks

The issue explicitly allows project/build/language checks as optional bounded hooks.

If hooks are implemented in this milestone, they must have a narrow contract such as:

```text
input: verified filesystem state + explicit bounded hook configuration
output: success / structured verification failure
```

Requirements:

- opt-in, not hidden behavior
- explicit timeout/resource bounds where execution is involved
- deterministic result representation
- cancellation/error handling
- no unbounded subprocess execution
- no implicit compiler/test-runner orchestration
- no policy bypass
- no transport-specific implementation

Examples of future hooks may include syntax validation or a lightweight project check, but do not build a generic command execution framework here.

If hooks are not needed for the current architecture, leave the extension point out of production code and document the boundary rather than speculative abstraction.

---

## 18. Security requirements

Preserve existing fail-closed filesystem security.

Verification must not:

- bypass canonical path validation
- follow an unsafe symlink outside the workspace
- accept absolute paths where the canonical editor rejects them
- introduce a second path-resolution implementation
- read arbitrary paths supplied by an untrusted verification result
- expose full sensitive file contents in errors/logs

Reuse existing secure filesystem helpers and workspace containment rules.

Verification is part of the mutation pipeline, not a reason to create a security exception after mutation.

---

## 19. Performance and resource limits

Verification necessarily performs additional filesystem I/O. Implement it predictably.

Requirements:

- do not read the same file repeatedly without reason
- avoid retaining unnecessary full-file copies after verification
- reuse already-observed bytes only for optimization, never as the authoritative state if the filesystem must be re-read
- keep hashes and metadata bounded
- avoid unbounded diagnostic payloads
- do not make project-wide verification the default

For multi-file transactions, verify affected files only.

Large files must not trigger accidental quadratic processing.

---

## 20. Tests required

Add focused unit and integration tests around the actual edit service.

At minimum cover:

### Success

- replace → verified → committed
- insert → verified → committed
- delete-range → verified → committed
- patch → verified → committed
- multi-file successful verification

### State capture

- actual after hash is correct
- actual after byte size is correct
- actual after line count is correct
- after-state reflects filesystem contents, not only candidate contents

### Semantic verification

- intended replacement confirmed
- intended insertion confirmed
- intended deletion confirmed
- no-op semantics are deterministic
- unexpected content fails verification
- unexpected target existence/deletion state fails verification

### I/O failures

- missing target
- unreadable target where testable
- read/stat failure
- verification I/O failure

### Failure injection

- external mutation after apply
- external deletion after apply
- corrupted resulting bytes
- verification failure after a multi-file mutation

### Rollback

- verification failure triggers AWE-006 rollback
- original bytes are restored
- rollback success does not convert verification failure into success
- rollback failure is surfaced distinctly
- multi-file rollback restores every affected file when recovery is valid

### Lifecycle

- `Applied` cannot be treated as final success
- `Verified` only occurs after real filesystem verification
- `Committed` cannot occur after verification failure
- failure states serialize deterministically if serialization is already part of the service contract

### Compatibility

Run all existing AWE-001 through AWE-007 tests and ensure their semantics remain intact.

---

## 21. Testing methodology

Prefer tests that exercise the real filesystem and real application-service path rather than testing a duplicate fake editor.

Use temporary workspaces/directories for integration tests.

For concurrency/race behavior, use deterministic barriers/hooks where possible instead of timing sleeps.

For failure injection, ensure the test proves the verifier inspected the resulting filesystem rather than merely accepting the candidate buffer.

Avoid tests that only assert an internal helper returned `Ok(())` without checking disk state.

---

## 22. Interface and architecture constraints

The verification implementation must remain below transport interfaces.

Do not implement separate verification logic in:

- MCP handlers
- CLI commands
- TUI actions
- Control API handlers

All interfaces must eventually call the same application-service semantics.

Do not modify public transport contracts unless absolutely necessary for the shared service result to be representable. If a transport change is required, keep it minimal and backward compatible.

---

## 23. Explicit non-goals for AWE-008

Do **not** implement or redesign:

- AWE-009 MCP agent-grade editing exposure
- AWE-010 edit-level rollback API
- AWE-011 capability-policy edit paths
- AWE-012 snapshots/provenance
- AWE-013 persistent audit
- AWE-014 CLI agent-grade editing
- AWE-015 full editing test-suite milestone beyond tests required here
- AWE-016 real MCP-client validation
- AWE-017 complete acceptance workflow
- AWE-018 editing contract finalization
- agent profiles/sessions
- worktree isolation
- collaboration
- remote/cloud execution
- generic build/test orchestration
- model routing
- a new filesystem abstraction unrelated to verification
- a second edit engine
- a second rollback implementation

If a later feature is necessary for correctness, implement only the smallest compatibility seam and document it; do not pull the later milestone into AWE-008.

---

## 24. Documentation discipline

Update documentation only if required to accurately describe the newly implemented AWE-008 behavior and only within the implementation scope.

Do not rewrite roadmap documents merely to mark the issue complete unless the repository's established workflow requires it.

Do not claim a command/interface is implemented if only the underlying service exists.

Do not document verification as crash-proof, race-free, or compiler-grade unless tests prove those guarantees.

---

## 25. Code quality requirements

Use idiomatic Rust:

- clear ownership boundaries
- `Result`/typed errors instead of panic-driven control flow
- minimal cloning
- deterministic behavior
- small focused helpers
- explicit lifecycle transitions
- no unnecessary dependencies
- no dead-code abstraction introduced solely for hypothetical future features

Preserve serialization compatibility for existing edit models unless a narrowly justified change is required.

Comments should explain invariants and non-obvious filesystem behavior, not restate obvious code.

---

## 26. Verification of the implementation itself

Before declaring AWE-008 complete, run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Also perform targeted verification such as:

```bash
cargo test verification
cargo test edit
```

Use the repository's actual test/module names if these filters differ.

Inspect the final diff carefully:

```bash
git status --short
git diff --check
git diff
```

Confirm that:

- only files required for AWE-008 changed
- no unrelated formatting churn exists
- no debug code remains
- no test bypasses production verification semantics
- no future milestone was silently implemented

If the repository contains an established terminal-level acceptance path for editing, exercise the relevant local path where practical.

---

## 27. Definition of Done

AWE-008 is complete only when all of the following are true:

- [ ] Post-edit verification exists in the canonical edit service path.
- [ ] Verification runs by default for agent-grade mutations.
- [ ] Verification reads/observes the actual filesystem result.
- [ ] Target existence/deletion state is verified.
- [ ] Text results are readable and semantically verified where applicable.
- [ ] The intended operation's postcondition is verified for each supported operation.
- [ ] Actual after hash/size/line count are captured.
- [ ] Verification failure is a structured failure.
- [ ] Verification failure can trigger existing AWE-006 rollback.
- [ ] Rollback-after-verification-failure is tested.
- [ ] Failure injection proves the verifier does not trust only the in-memory candidate.
- [ ] Multi-file verification is transaction-aware.
- [ ] Stale-state behavior remains owned by AWE-007 rather than duplicated.
- [ ] Security/path constraints remain fail-closed.
- [ ] Unicode/newline/EOF semantics remain correct.
- [ ] Optional hooks, if implemented, are bounded and explicitly contracted.
- [ ] The editor has not become a compiler/test/build runner.
- [ ] Existing tests remain green.
- [ ] `fmt`, `check`, `test`, and `clippy -D warnings` pass.
- [ ] Final diff is limited to AWE-008 implementation scope.

---

## 28. Final implementation report

At completion, report:

1. Files changed.
2. Verification service/functions added or modified.
3. Operation-specific postconditions implemented.
4. Actual after-state capture behavior.
5. Structured verification errors.
6. AWE-006 rollback integration.
7. Failure-injection tests.
8. Multi-file verification behavior.
9. Security/TOCTOU limitations.
10. Test commands and results.
11. Any remaining limitation that is genuinely outside AWE-008.

Do not claim stronger guarantees than the tests demonstrate.

---

## 29. Hard STOP rule

After AWE-008 is implemented, tested, and verified:

**STOP.**

Do not continue to AWE-009 or any subsequent issue.

Do not modify the AWE-009 prompt or any later issue-resolving prompt.

Do not implement MCP exposure merely because AWE-009 depends on this work.

The task is complete when the AWE-008 implementation and its required tests satisfy the Definition of Done.

**Final instruction: implement only AWE-008 / GitHub issue #29, preserve the existing architecture and earlier AWE semantics, verify the actual filesystem result before success, integrate existing rollback on verification failure, run the full Rust quality gates, inspect the final diff, and STOP.**
