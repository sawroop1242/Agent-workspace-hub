# AWE-003 — Implement Line-Range Insert and Delete Operations

## Issue
GitHub #24 — **AWE-003: Implement line-range insert and delete operations**

## Dependency
Blocked by **#22 — AWE-001: Design the canonical Edit Transaction model**.

This prompt assumes the canonical edit domain model from AWE-001 already exists. Do not redesign or replace that model unless the repository proves that a narrowly scoped correction is required.

---

## Objective

Implement the next two safe editing primitives in AWH:

- `insert` — insert UTF-8 text at a deterministic line boundary.
- `delete_range` — delete an inclusive line range.

These operations must use the canonical `EditTransaction` / `EditOperation` model introduced by AWE-001 and fit the existing AWH service architecture.

The goal is **controlled agent-grade editing**, not another raw `FilesService.write()` wrapper.

The implementation must validate the complete requested mutation before changing the file and must never silently overwrite a stale file state.

---

# 1. Read the repository before editing

Before changing code, inspect:

1. `src/services/edit.rs`
2. `src/services/files.rs`
3. `src/services/mod.rs`
4. Existing service error conventions.
5. Existing filesystem tests and temporary-directory test patterns.
6. `Cargo.toml` and currently enabled dependencies.
7. AWE-001 implementation and any tests added for `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, and `EditError`.

Do not assume the exact AWE-001 API from this prompt. Adapt to the implementation actually present in the repository while preserving its architectural intent.

The existing `FilesService` is the workspace security boundary. Reuse it rather than duplicating path traversal or symlink-escape logic. `FilesService` already validates paths relative to the project root and enforces the project file-size limit. fileciteturn45file0

---

# 2. Architectural rule

AWH is **MCP-first infrastructure, not an agent framework**.

For this issue:

```text
Agent
  ↓
future MCP / CLI / Control API
  ↓
EditService
  ↓
FilesService / filesystem
```

Do not implement MCP tools, CLI commands, TUI commands, agent reasoning, snapshots, audit, policy engine, rollback, Git integration, or multi-agent behavior in this issue.

The core editing service must remain transport-independent so all future interfaces can call the same implementation.

---

# 3. Required operations

Implement the behavior represented by the canonical AWE-001 operations:

```text
EditOperation::Insert
EditOperation::DeleteRange
```

Use the exact field names/types already established by AWE-001 where possible.

Do not introduce a second competing edit request model.

## Insert semantics

Support deterministic insertion at a line boundary.

The operation must define:

- target relative path
- insertion line/index
- inserted UTF-8 text
- expected current state when supplied

The API must clearly define whether the insertion point means:

- before the specified line, or
- after the specified line

Choose one deterministic convention consistent with AWE-001 and document it in code/tests. Prefer **insert before the specified 1-based line** for existing lines, with an explicit convention for inserting at EOF.

Do not create ambiguous zero-based/one-based behavior.

## Delete-range semantics

Support deletion of an **inclusive** line range:

```text
start_line..=end_line
```

The operation must define:

- target relative path
- 1-based inclusive start line
- 1-based inclusive end line
- expected current state when supplied

Reject:

- zero line numbers
- `start_line > end_line`
- ranges outside the file
- invalid line boundaries

Define and test behavior for the final line and EOF explicitly.

---

# 4. Preserve the canonical EditTransaction model

The implementation must integrate with the AWE-001 model rather than bypass it.

Expected conceptual flow:

```text
EditTransaction requested
        ↓
validate operation structure
        ↓
resolve target through FilesService
        ↓
read current state
        ↓
validate ExpectedState
        ↓
validate line boundaries
        ↓
construct resulting content in memory
        ↓
validate resulting content/size
        ↓
perform one controlled mutation
        ↓
re-read and verify resulting state
        ↓
return transaction/result with before/after state
```

Do not mark an edit as successfully applied merely because a write call returned `Ok(())`.

Use the existing `EditStatus` states and `EditError` variants where they already exist.

If AWE-001's model does not yet expose a helper required by this operation, add the smallest model-compatible change necessary. Do not redesign unrelated parts of the model.

---

# 5. Expected-state / stale-state protection

This is a critical safety invariant.

Before mutation, compare the current file state against the transaction's `ExpectedState` when one is supplied.

At minimum support the fields already defined by AWE-001, such as:

- SHA-256
- byte size
- line count
- contextual state where applicable

If the expected state does not match the actual state:

```text
NO MUTATION
→ return structured edit conflict
```

Never silently apply an edit to a file that changed after the agent read it.

The operation must therefore be safe against this sequence:

```text
Agent reads file
        ↓
Another process modifies file
        ↓
Agent submits insert/delete request with old state
        ↓
AWH detects mismatch
        ↓
Edit rejected
        ↓
File remains unchanged by AWH
```

Add an integration test for this exact workflow.

---

# 6. Line parsing rules

Implement line handling deliberately.

Consider at least:

- empty file
- one-line file
- multi-line file
- trailing newline
- no trailing newline
- CRLF input
- LF input
- empty lines
- final line without newline
- insertion at first line
- insertion at last line
- insertion at EOF
- deletion of one line
- deletion of multiple lines
- deletion including first line
- deletion including last line
- deletion of the complete file

Do not blindly use `.lines()` if doing so loses information required to preserve newline semantics.

The implementation should avoid unnecessary newline normalization.

If the existing AWH conventions define newline behavior, follow them. Otherwise preserve the existing file's newline style whenever practical and document the chosen behavior.

---

# 7. UTF-8 requirements

AWH's current file service works with UTF-8 text files.

The implementation must:

- operate on valid UTF-8 text
- preserve Unicode characters correctly
- never calculate line positions using byte offsets incorrectly
- test multi-byte characters such as Hindi/Devanagari and emoji

Example test content should include something similar to:

```text
पहली लाइन
दूसरी लाइन 🚀
तीसरी लाइन
```

Line operations must operate on logical lines, not character-byte assumptions.

---

# 8. Mutation safety

Before writing:

1. Resolve and authorize the path through the existing filesystem service.
2. Read the current content.
3. Compute/obtain the current `FileState`.
4. Validate `ExpectedState`.
5. Validate the operation's line boundaries.
6. Construct the complete new content in memory.
7. Verify the resulting content remains within the existing file-size limit.
8. Only then perform the mutation.

If any validation fails:

```text
original file == unchanged file
```

There must be **zero mutation on validation failure**.

Do not partially write the file while discovering an invalid line range.

---

# 9. Atomicity boundary

AWE-003 does not need to implement the complete AWE-006 transaction/rollback system.

However, the individual file mutation must be performed as safely as the current service architecture permits.

Use the existing filesystem write abstraction where appropriate, and do not bypass `FilesService` security checks merely to implement the edit.

Do not introduce a second atomic-write implementation if the repository already has one.

If full transaction-level atomicity is intentionally deferred to #27/AWE-006, document that boundary clearly rather than pretending this issue implements multi-operation rollback semantics.

---

# 10. Result/state information

A successful operation should expose enough information for later AWE-005/AWE-006/AWE-008 work.

At minimum preserve or return:

- edit ID
- operation type
- target path
- before state/hash
- after state/hash
- resulting edit status

Use the canonical AWE-001 types instead of creating duplicate result structures where possible.

Do not add snapshot IDs, audit IDs, agent IDs, session IDs, or provenance graphs yet unless those types already exist and are explicitly required by the current architecture.

---

# 11. Error behavior

Use structured errors consistent with AWE-001.

At minimum distinguish:

- invalid operation
- invalid line range
- target not found
- target not a text file
- expected-state conflict
- file too large
- invalid UTF-8 if encountered
- read failure
- write failure
- post-write verification failure

Do not return generic success/failure strings when an existing structured error type can represent the failure.

Error messages should be useful to an agent but must not expose secrets or unnecessary file contents.

---

# 12. Tests — mandatory

Add both unit and filesystem integration coverage.

## Insert tests

Test:

- insert before first line
- insert between two lines
- insert at last valid boundary
- insert at EOF
- insert into empty file
- insert Unicode text
- insert with LF
- insert with CRLF where supported
- invalid line number
- missing file
- oversized resulting content
- expected hash mismatch
- expected size mismatch
- expected line-count mismatch
- validation failure leaves file unchanged

## Delete tests

Test:

- delete one line
- delete multiple lines
- delete first line
- delete last line
- delete first through last line
- delete invalid zero line
- delete inverted range
- delete range beyond EOF
- delete from empty file
- Unicode content
- trailing newline
- no trailing newline
- expected-state mismatch
- validation failure leaves file unchanged

## Security tests

Verify:

- absolute path rejected
- `../` traversal rejected
- nested traversal rejected
- symlink escape remains blocked through `FilesService`

Do not duplicate the entire `FilesService` security test suite; add only integration coverage proving the edit path cannot bypass it.

## Stale-state test

Mandatory scenario:

```text
create file
→ capture ExpectedState
→ modify file externally
→ attempt insert/delete with stale ExpectedState
→ expect conflict
→ assert file equals externally modified content
```

## Invariant test

For every rejected request:

```text
state_after == state_before
```

For every successful request:

```text
reported_after_state == actual_file_state
```

---

# 13. Property/fuzz-style testing where practical

If the repository already uses `proptest` or another property-testing framework, add focused properties.

Useful invariants include:

- valid insertion never removes existing content except where required by newline representation
- valid deletion removes exactly the requested logical line range
- invalid ranges never mutate the file
- Unicode input remains valid UTF-8
- reported hashes match actual file hashes

Do not introduce a heavy new dependency solely for this issue unless clearly justified.

---

# 14. Service architecture requirements

Keep the implementation in the service layer.

Prefer:

```text
src/services/edit.rs
```

or the existing edit-service module established by AWE-001.

If helper functions are needed, keep them private unless another existing service genuinely needs them.

`src/services/mod.rs` should continue exposing the edit service through the existing module structure.

Do not place business logic in:

- MCP handlers
- CLI command handlers
- TUI widgets
- HTTP handlers

Those integrations belong to later issues.

The service layer remains the single owner of edit semantics.

---

# 15. Do not implement these items

Strictly out of scope for AWE-003:

- `filesystem.patch` multi-operation transaction (#25)
- unified diff support (#26)
- full transaction atomicity/rollback (#27)
- advanced conflict system beyond this operation's expected-state validation (#28)
- post-edit verification framework beyond the minimal correctness verification needed here (#29)
- MCP editing tools (#30)
- edit rollback (#31)
- capability/policy integration (#32)
- snapshot/provenance (#33)
- audit events (#34)
- CLI editing commands (#35)
- complete editing test suite (#36)
- real MCP client interoperability (#37)
- end-to-end acceptance workflow (#38)
- editing documentation contract (#39)

Do not solve future issues prematurely.

---

# 16. Compatibility with AWE-001

Before implementation, verify the exact canonical definitions from AWE-001.

The implementation must:

- reuse `EditId`
- reuse `EditOperation::Insert` / `DeleteRange`
- reuse `ExpectedState`
- reuse `FileState`
- reuse `EditStatus`
- reuse `EditError`
- reuse the canonical transaction structure

If a type currently has a slightly different name or field shape, adapt to the repository rather than creating a duplicate type.

If the AWE-001 implementation contains a genuine defect that blocks AWE-003, make the smallest backward-compatible correction and explicitly report it.

---

# 17. Verification commands

Run all applicable project checks:

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

If the repository's current toolchain makes one command unavailable, report the exact failure instead of claiming success.

Also run the focused edit tests independently so failures are easy to diagnose.

---

# 18. Final code-review checklist

Before considering AWE-003 complete, verify:

- [ ] AWE-001 canonical model is reused.
- [ ] Insert operation is implemented.
- [ ] Delete-range operation is implemented.
- [ ] Line semantics are explicit and deterministic.
- [ ] 1-based line behavior is documented/tested.
- [ ] Inclusive deletion range is documented/tested.
- [ ] EOF behavior is explicit.
- [ ] Trailing newline behavior is tested.
- [ ] CRLF/LF behavior is considered.
- [ ] Unicode/UTF-8 behavior is tested.
- [ ] Expected state is checked before mutation.
- [ ] Stale state produces a structured conflict.
- [ ] Zero matches/invalid ranges cannot mutate files.
- [ ] Resulting file size is validated before write.
- [ ] Existing `FilesService` path security is reused.
- [ ] No full-file rewrite API is exposed as the agent-facing abstraction.
- [ ] Before/after state is accurate.
- [ ] Post-write state matches the reported state.
- [ ] Tests cover success and failure paths.
- [ ] No future issue's scope has been pulled into AWE-003.
- [ ] Formatting/check/test/clippy results are known.

---

# 19. Definition of Done

AWE-003 is complete only when an agent can safely perform:

```text
read file
  ↓
construct Insert/DeleteRange transaction
  ↓
validate expected state
  ↓
validate line boundaries
  ↓
construct new content
  ↓
perform controlled write
  ↓
verify resulting file state
  ↓
return canonical edit result
```

And when the following invariant holds:

```text
invalid request
    → structured error
    → zero mutation
```

while:

```text
valid request + matching expected state
    → deterministic mutation
    → correct before/after state
```

The implementation must be suitable for later composition by AWE-004 (`filesystem.patch`) without requiring a redesign of the canonical edit model.

---

# 20. Recommended commit

Use a focused commit such as:

```text
feat(edit): add safe line-range insert and delete operations
```

Keep the commit limited to AWE-003 and directly required tests/helpers.

---

# Final implementation report

After implementation, report:

1. Files changed.
2. Exact `Insert` and `DeleteRange` behavior.
3. How line numbering and EOF are handled.
4. How newline preservation is handled.
5. How expected-state conflicts are detected.
6. How filesystem security is reused.
7. Tests added.
8. Commands executed and their actual results.
9. Any AWE-001 compatibility adjustment made.
10. Any intentionally deferred behavior for #25–#39.
11. Commit SHA.

Do not claim AWE-003 is complete if the required checks or acceptance criteria are failing.