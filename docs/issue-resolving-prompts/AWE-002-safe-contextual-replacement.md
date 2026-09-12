# AWE-002 — Safe Contextual Replacement

**GitHub issue:** #23  
**Milestone:** Agent-Grade Editing  
**Priority:** P0  
**Depends on:** #22 / AWE-001 — Canonical Edit Transaction Model  
**Status:** Implementation prompt

## Master implementation prompt

You are implementing **AWE-002 / GitHub Issue #23: Implement safe contextual replacement** in the AWH Rust repository.

The goal is to turn the canonical AWE-001 edit transaction model into the first real, agent-grade file mutation primitive: a **safe contextual replacement** operation that can replace text only when the requested file state and edit context are still valid.

This is a core infrastructure change. AWH is MCP-first infrastructure, not an AI agent. The implementation must contain no LLM reasoning, agent planning, prompt logic, or model-provider code.

The replacement primitive must never silently overwrite a newer file state and must never mutate the file when validation fails.

---

## 1. First inspect the repository

Before changing code, inspect the current `rust` branch and existing architecture.

At minimum inspect:

- `AGENTS.md`
- `Cargo.toml`
- `src/services/mod.rs`
- `src/services/edit.rs`
- `src/services/files.rs`
- existing error conventions
- existing filesystem/security tests
- existing atomic-write helpers, if any
- existing hashing/file-state helpers, if any
- existing service-layer tests
- any current editor/replacement implementation already present

The canonical AWE-001 model already exists in `src/services/edit.rs`. Extend it rather than replacing it.

AWE-001 currently provides the shared vocabulary around:

- `EditId`
- `EditOperation::Replace`
- `ExpectedState`
- `FileState`
- `EditStatus`
- `EditTransaction`
- `EditError`
- SHA-256 file-state calculation
- structural transaction validation

Treat those types as the canonical edit-domain contract. Do not create a second competing transaction model.

`FilesService` remains responsible for workspace-root/path security and normal filesystem access. Reuse it instead of duplicating path traversal and symlink protection.

---

## 2. Objective

Implement a safe replacement service for the AWE-001 `EditOperation::Replace` operation.

The conceptual flow is:

```text
EditTransaction
      ↓
structural validation
      ↓
resolve/read through FilesService
      ↓
observe current FileState
      ↓
validate ExpectedState
      ↓
locate exact contextual match
      ↓
validate occurrence semantics
      ↓
construct replacement content in memory
      ↓
validate resulting content
      ↓
atomic filesystem mutation
      ↓
observe after FileState
      ↓
return structured result
```

The implementation must guarantee:

> **No validation failure may mutate the target file.**

And:

> **A stale expected state must result in a conflict rather than an overwrite.**

---

## 3. Scope

Implement the core safe replacement primitive only.

The implementation should live in the existing service layer, preferably by extending `src/services/edit.rs` or by introducing a narrowly scoped `EditService` module such as `src/services/editor.rs` only if that better matches the repository architecture.

If a new module is introduced, register it through `src/services/mod.rs` and keep the canonical AWE-001 domain types in their existing location.

The service should expose a transport-independent API that can later be called by:

- MCP
- CLI
- TUI
- Control API
- internal service code

Do **not** implement transport-specific MCP tools or CLI/TUI commands in this issue unless a very small internal test adapter is required.

Do **not** implement:

- insert
- delete-range
- multi-operation patch
- unified diff application
- rollback
- snapshot persistence
- provenance persistence
- audit event infrastructure
- capability/policy framework changes
- Git worktrees
- agent/session management
- multi-agent functionality

Those belong to later issues.

---

## 4. Canonical operation

The implementation must consume the existing AWE-001 operation:

```rust
EditOperation::Replace {
    path,
    old,
    new,
    occurrence,
}
```

Do not introduce another replacement request type unless an adapter is strictly required at the service boundary.

Interpret the fields as follows:

- `path`: workspace-relative target path
- `old`: exact contextual text to locate
- `new`: replacement text
- `occurrence`: optional expected occurrence selector/count according to the existing AWE-001 semantics

If the current AWE-001 semantics are ambiguous, inspect the code and choose the narrowest behavior that preserves safe editing. Document the decision in code/tests rather than silently inventing broad behavior.

---

## 5. Safe matching semantics

Replacement must be **exact and deterministic**.

Do not:

- perform fuzzy matching
- normalize arbitrary whitespace
- ignore case
- perform regex replacement unless explicitly represented by the canonical model
- silently select the first match when multiple matches exist
- silently replace every occurrence unless that behavior is explicitly requested

At minimum support these safe outcomes:

### Zero matches

If `old` does not occur in the current file:

- return a structured replacement/match error
- do not mutate the file
- do not create a new file

### Exactly one match

If `old` occurs exactly once and the request permits a single replacement:

- replace that occurrence
- continue through verification

### Multiple matches

If `old` occurs more than once:

- do not guess which occurrence the agent intended
- require explicit occurrence semantics
- return a structured error when the requested occurrence constraint is not satisfied
- do not mutate the file

The behavior must be deterministic and covered by tests.

---

## 6. Occurrence semantics

Use the existing `Option<usize>` field from AWE-001 rather than introducing a new request parameter.

Define and document one unambiguous meaning for it.

Recommended safe semantics:

- `None`: replacement is allowed only when exactly one match exists
- `Some(n)`: `n` identifies the one-based occurrence that should be replaced, while the service still verifies that the requested occurrence exists

If the repository's current AWE-001 tests or conventions establish a different meaning, preserve those conventions instead.

Do not create behavior where `None` means "replace all" because that is unsafe for agent-grade editing.

If `Some(n)` is used as an occurrence selector, return a structured error when `n == 0` or when the requested occurrence does not exist.

Add explicit tests for the chosen semantics.

---

## 7. Expected-state / stale-state protection

Before constructing or applying the replacement, observe the current file state.

Use the canonical `FileState` and `ExpectedState` model from AWE-001.

The service must compare the caller's expected state with the actual current state.

At minimum support:

- expected SHA-256 mismatch
- expected size mismatch
- expected line-count mismatch where supplied

A mismatch must produce a **conflict**, not a generic write failure.

The service must never continue to mutation after an expected-state conflict.

If AWE-001 exposes `ExpectedState.context`, use it as an additional contextual precondition where appropriate. Do not treat context as a replacement for the complete-file hash when a hash was explicitly supplied.

The resulting error/status should make it possible for a future MCP client to distinguish:

```text
edit rejected because the file changed
```

from:

```text
edit rejected because the requested text was not found
```

These are different failure classes and must not be collapsed into one generic error.

---

## 8. File access and security

All file access must remain inside the existing AWH workspace security boundary.

Reuse `FilesService` for:

- path validation
- workspace-root enforcement
- traversal prevention
- symlink escape protection
- file size limits
- UTF-8 reads/writes

Do not duplicate `resolve_checked` or create a second workspace-root security implementation.

Do not bypass `FilesService` merely because direct `std::fs` access appears simpler.

If atomic replacement requires lower-level filesystem primitives, isolate that implementation carefully and preserve the same path-security guarantees established by `FilesService`.

The implementation must reject:

- absolute paths
- `..` traversal
- paths escaping the workspace
- symlink-based workspace escapes
- oversized text files according to existing limits
- unreadable/non-UTF-8 content where the existing service treats it as unsupported

Add or reuse security tests rather than weakening the existing filesystem boundary.

---

## 9. Construct the new content before mutation

Never mutate the file while searching for the match.

The service should:

1. read the current content
2. validate expected state
3. locate the intended occurrence
4. construct the complete resulting content in memory
5. validate the resulting content/size
6. only then perform the filesystem mutation

This is required to guarantee zero mutation on validation failure.

Do not perform a sequence such as:

```text
open → truncate → start replacing
```

before all validation has succeeded.

---

## 10. Atomic write requirement

A successful replacement must be atomic from the perspective of readers as far as the platform/filesystem implementation permits.

Do not simply call the existing non-atomic `FilesService::write` if that would expose a partially written file or truncate the original before the new content is ready.

Implement or reuse a narrowly scoped atomic-write mechanism.

A robust approach may be:

```text
validate target
    ↓
write complete new content to temporary file in same directory
    ↓
flush/sync as appropriate for existing AWH reliability conventions
    ↓
rename temporary file over target
    ↓
cleanup temporary file on failure
```

The implementation must consider:

- same-directory temporary file placement
- rename semantics on supported platforms
- preservation of file contents on failed replacement
- cleanup of temporary files
- file size limits
- Android/Termux compatibility

Do not introduce a large cross-platform abstraction for this issue if a small, well-tested helper is sufficient.

If true filesystem-level atomicity differs by platform, document the guarantee and test the supported target platforms as far as the repository's CI allows.

---

## 11. Before/after state

A successful replacement must calculate both:

- `before` state
- `after` state

using the canonical `FileState` representation.

The result should expose at least:

- `edit_id`
- target path
- replacement status
- before hash
- after hash
- occurrence information
- bytes/metadata where useful

Do not introduce a new parallel file-state structure.

The before hash must correspond to the exact content that was validated immediately before mutation.

The after hash must correspond to the content actually observed after mutation.

The hashes must differ for a successful non-no-op replacement unless the replacement is intentionally identical.

---

## 12. Edit transaction lifecycle

Update the canonical `EditTransaction` lifecycle only through states that are actually justified by this issue.

The intended successful progression is approximately:

```text
Requested
  → Authorized
  → Located
  → Validated
  → Applied
  → Verified
  → Committed
```

However, AWE-002 does **not** implement the complete capability, snapshot, audit, or provenance pipeline yet.

Do not fake `Authorized` or `Snapshotted` behavior merely to make the enum look complete.

If the current service boundary cannot legitimately perform a lifecycle transition yet, keep the operation result scoped to what AWE-002 owns and leave later integration points explicit.

Failure should be distinguishable as appropriate:

- `Conflict`
- `ValidationFailed`
- `ApplyFailed`
- `VerificationFailed`

Do not claim rollback support in this issue.

---

## 13. Structured errors

Extend `EditError` or the appropriate existing AWH error abstraction with narrowly scoped replacement errors.

At minimum distinguish:

- file read failure
- file missing/not found where distinguishable
- zero matches
- unexpected match count
- invalid occurrence selector
- requested occurrence not found
- expected hash mismatch
- expected size mismatch
- expected line-count mismatch
- contextual precondition mismatch
- resulting content exceeds file-size limit
- atomic-write failure
- post-write verification failure

Avoid string-only error handling when callers need to branch on failure type.

Do not use `anyhow` as the public domain error abstraction if the service layer can expose structured edit errors while using `anyhow` internally for filesystem context.

Preserve useful underlying error context where appropriate.

---

## 14. Verification after mutation

After the atomic write completes, read/observe the file again and calculate its `FileState`.

Verify:

1. target still exists
2. resulting content is readable as expected
3. the requested replacement is present
4. the intended old occurrence is no longer present at the replaced location
5. after-state hash matches the actual resulting content
6. before/after state is internally consistent

If post-write verification fails, return a structured verification failure.

Do not silently report success merely because the filesystem rename/write returned successfully.

AWE-002 does not yet implement rollback-on-verification-failure unless the existing architecture already provides a safe reusable rollback primitive. If no such primitive exists, leave rollback to AWE-010 and make the failure explicit rather than inventing partial rollback behavior.

---

## 15. No-op replacement

Handle `old == new` explicitly.

A replacement that produces identical content must not be treated as a normal mutation without consideration.

Prefer:

- validate the request
- detect that resulting content equals current content
- return a deterministic no-op result
- do not rewrite the file unnecessarily

Do not change the before/after hash when no content changes.

Document and test the chosen behavior.

---

## 16. Newline and UTF-8 behavior

Replacement operates on UTF-8 text, consistent with `FilesService`.

Do not silently normalize:

- CRLF → LF
- LF → CRLF
- trailing newline state
- Unicode normalization
- tabs/spaces

The replacement must preserve all untouched bytes/content exactly.

Tests must include:

- LF files
- CRLF files where supported by the text representation
- final newline present
- final newline absent
- Unicode text
- multiline replacement text
- empty replacement text
- empty file

Be careful with Rust `str::lines()` semantics when deriving line counts; reuse AWE-001's existing `FileState` semantics unless there is a clear bug that this issue must address.

---

## 17. Required service API shape

Create a small transport-independent API similar in spirit to:

```rust
pub struct EditService {
    files: FilesService,
}

impl EditService {
    pub fn new(files: FilesService) -> Self;

    pub fn replace(&self, transaction: EditTransaction) -> Result<EditResult, EditError>;
}
```

The exact API may differ to match existing async/service conventions.

Important requirements:

- consume the canonical AWE-001 model
- return a structured result
- do not expose MCP-specific types
- do not expose CLI/TUI types
- do not embed agent reasoning
- do not bypass `FilesService`

If AWH's service architecture requires async methods, follow the repository's established convention rather than introducing synchronous APIs inconsistently.

---

## 18. Result model

Introduce a narrowly scoped result type only if the existing architecture does not already provide one.

A useful result should contain enough information for future transports to report:

- `edit_id`
- final status
- path
- whether the operation changed content
- occurrence selected/replaced
- before `FileState`
- after `FileState`

Do not add MCP response envelopes, JSON-RPC fields, CLI formatting, or UI state to this domain result.

Keep it serializable if that matches the AWE-001 domain convention.

---

## 19. Tests

Tests are a major part of this issue. Do not stop at unit tests of a string replacement helper.

### Unit tests

Add focused tests for:

1. exactly one match succeeds
2. zero matches fail
3. multiple matches with `occurrence == None` fail
4. explicit valid occurrence succeeds
5. occurrence zero fails
6. out-of-range occurrence fails
7. expected hash matches and replacement succeeds
8. expected hash mismatch returns conflict
9. expected size mismatch returns conflict
10. expected line count mismatch returns conflict
11. contextual precondition mismatch returns conflict/validation failure as designed
12. empty `old` is rejected
13. empty `new` is allowed and deletes the matched text
14. `old == new` returns a deterministic no-op
15. Unicode replacement succeeds
16. multiline replacement succeeds
17. untouched content remains identical
18. before/after hashes are correct
19. after-state is observable and consistent
20. structured errors identify the correct failure class

### Filesystem integration tests

Use `tempfile` and a real workspace directory.

Test:

- successful replacement of a real file
- missing file
- invalid relative path
- traversal path
- symlink escape where supported
- oversized file/content behavior
- no mutation after zero-match failure
- no mutation after occurrence failure
- no mutation after expected-state conflict
- no mutation after invalid input
- atomic replacement success
- failed atomic replacement preserves original content
- temporary-file cleanup after failure

### Concurrency/stale-state test

At minimum simulate:

```text
read expected state
external modification
attempt replacement with old expected hash
→ conflict
→ file remains externally modified
```

The test must prove that the stale request does not overwrite the external change.

### Property/invariant tests where practical

Test the key invariant:

```text
if validation fails:
    file_before == file_after
```

And for a successful replacement:

```text
after = replace(before, selected_old, new)
```

with the untouched portions preserved exactly.

---

## 20. Security tests

Explicitly verify that replacement cannot escape the workspace through:

- `../`
- absolute paths
- platform-specific prefix/root components
- symlink targets outside the workspace

Do not duplicate the full `FilesService` security test suite unnecessarily; add regression tests proving the new replacement path actually uses the existing security boundary.

A replacement request that fails path authorization must result in **zero mutation**.

---

## 21. Atomicity test strategy

Do not claim atomicity solely because the implementation uses a helper named `atomic_write`.

Tests should demonstrate that:

- original content is preserved if preparation fails
- original content is preserved if the temporary write fails
- original content is preserved if validation fails
- temporary artifacts are cleaned up
- successful replacement exposes the complete resulting content

Where deterministic failure injection is practical, add a small test-only mechanism rather than relying only on random OS failures.

Do not introduce a production fault-injection framework solely for this issue.

---

## 22. Architecture constraints

Preserve these AWH architectural invariants:

### MCP-first infrastructure

AWE-002 creates infrastructure that MCP will consume later. It must not become an MCP implementation itself.

### Agent-agnostic

The service must not know about OpenCode, OpenHands, Claude Code, Qwen Code, Codex, or any other agent.

### One canonical edit model

Use AWE-001's `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, and `EditStatus` rather than introducing competing models.

### Service-layer ownership

Business logic belongs in the service layer so MCP, CLI, TUI, and Control API can consume the same behavior later.

### FilesService remains the filesystem boundary

Do not bypass or duplicate its path/symlink/size protections.

### No full-file rewrite API

This milestone is specifically intended to give coding agents a precise edit primitive instead of requiring them to send whole-file rewrites.

### No silent overwrite

Stale state must be detected and rejected.

### No hidden mutation

Every validation failure must leave the original file unchanged.

---

## 23. Reuse existing infrastructure

Before adding helpers, search for existing implementations of:

- SHA-256 hashing
- file-state calculation
- file size limits
- UTF-8 reads/writes
- path security
- atomic writes
- structured errors
- service construction
- test workspace fixtures

Prefer reuse over duplication.

If an existing helper is unsafe for agent-grade editing, do not silently use it just because it exists. Explain why a new narrow helper is required.

If the new atomic-write helper belongs in a more general filesystem abstraction, keep its API small enough that AWE-004/AWE-006 can reuse it without redesigning the edit service.

---

## 24. Keep the change narrow

Do not solve the entire Agent-Grade Editing milestone in #23.

Do not implement:

- `filesystem.patch`
- `filesystem.insert`
- `filesystem.delete_range`
- `filesystem.apply_diff`
- rollback
- snapshots
- provenance persistence
- audit events
- policy engine redesign
- MCP registration
- CLI commands
- TUI commands
- Git integration
- multi-agent editing

The purpose of AWE-002 is to establish the first safe mutation primitive on top of the AWE-001 canonical model.

---

## 25. Verification commands

Run all applicable repository checks:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Also run targeted tests for the new replacement service before the full suite where practical.

If a command fails:

1. inspect the actual failure,
2. determine whether it is caused by this change,
3. fix the relevant issue,
4. rerun the failed check,
5. do not claim completion while required checks remain failing.

Inspect the final diff and verify that unrelated files/features were not changed.

If Android/Termux-specific behavior can be tested in the current environment, run the relevant tests there as well. Do not claim Android compatibility from Linux-only tests.

---

## 26. Recommended commit

Use a focused commit message such as:

```text
feat(edit): add safe contextual replacement
```

Do not mix unrelated refactors into the commit.

---

## 27. Final implementation report

After implementation, report:

- files changed
- whether `EditService` was added or extended
- exact replacement semantics
- occurrence semantics
- stale-state/conflict behavior
- atomic-write strategy
- before/after state behavior
- verification behavior
- structured errors added
- security boundary reused
- tests added
- targeted test results
- full `cargo check` result
- full `cargo test` result
- clippy result
- formatting result
- Android/Termux verification result, if actually run
- architecture decisions
- unresolved issues
- explicit confirmation that later milestones such as rollback/snapshots/audit/MCP were not prematurely implemented

Never claim a command passed unless it was actually executed.

---

## Definition of Done

AWE-002 is complete only when all of the following are true:

- [ ] The canonical AWE-001 `EditOperation::Replace` is executable through a transport-independent service.
- [ ] Exactly-one-match replacement works.
- [ ] Zero-match requests fail without mutation.
- [ ] Ambiguous multiple matches fail unless explicit occurrence semantics select one.
- [ ] Invalid occurrence selectors fail without mutation.
- [ ] Expected SHA-256 mismatch produces a structured conflict.
- [ ] Expected size/line-count mismatches are handled safely when supplied.
- [ ] Contextual preconditions are enforced when supplied.
- [ ] No validation failure mutates the file.
- [ ] Replacement content is constructed before mutation.
- [ ] Successful writes use an atomic replacement strategy appropriate to supported platforms.
- [ ] Before and after `FileState` values are returned.
- [ ] Post-write verification is performed.
- [ ] No-op replacements are deterministic and do not unnecessarily rewrite the file.
- [ ] Existing `FilesService` security boundaries are reused.
- [ ] Path traversal and symlink escape are rejected.
- [ ] Unit and real-filesystem integration tests cover success, conflict, ambiguity, security, and failure paths.
- [ ] The stale-state test proves an external modification cannot be silently overwritten.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo check` passes.
- [ ] `cargo test` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] The final diff remains narrowly scoped to AWE-002.

The resulting service becomes the foundation for **AWE-003: Line-Range Insert and Delete Operations** and contributes the safe mutation/atomic-write primitives that later AWE-004, AWE-006, AWE-007, and AWE-008 will build upon.
