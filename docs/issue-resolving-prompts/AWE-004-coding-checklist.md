# AWE-004 — Coding Checklist

## `src/services/edit.rs`

### Preparation model

- [ ] Add internal `PreparedFileMutation`.
- [ ] Add internal `PreparedPatch` if useful.
- [ ] Preserve canonical AWE-001 models.

### Pure transformations

- [ ] Extract/reuse replace transformation.
- [ ] Extract/reuse insert transformation.
- [ ] Extract/reuse delete-range transformation.
- [ ] Add `apply_operation_to_content()` dispatcher.
- [ ] Ensure helpers never write to disk.

### Preparation

- [ ] Validate non-empty transaction.
- [ ] Validate every operation before mutation.
- [ ] Validate every target path through existing filesystem security.
- [ ] Load each affected file.
- [ ] Capture original `FileState`.
- [ ] Validate expected state.
- [ ] Compose same-file operations in transaction order.
- [ ] Calculate final content and `FileState`.
- [ ] Prepare all files before commit.

### Commit

- [ ] Commit only after complete preparation succeeds.
- [ ] Write one final state per affected file.
- [ ] Never write intermediate states.
- [ ] Reuse existing safe filesystem write behavior.
- [ ] Do not implement crash recovery here.

### Result/verification

- [ ] Populate before/after state.
- [ ] Preserve edit ID.
- [ ] Preserve operation ordering.
- [ ] Perform minimal post-write verification.
- [ ] Return structured failure information.

## `src/services/files.rs`

- [ ] Reuse `FilesService` path validation.
- [ ] Reuse workspace-root enforcement.
- [ ] Reuse symlink/traversal protections.
- [ ] Reuse file-size and UTF-8 handling.
- [ ] Avoid unrelated changes.

## Tests

### Basic

- [ ] Replace
- [ ] Insert
- [ ] Delete range
- [ ] Mixed operations
- [ ] Empty file

### Same-file composition

- [ ] Replace -> Replace
- [ ] Replace -> Insert
- [ ] Insert -> Replace
- [ ] Insert -> Delete
- [ ] Delete -> Insert
- [ ] Replace -> Delete

### Multi-file

- [ ] Two files succeed.
- [ ] Three files succeed.
- [ ] One final mutation per affected file.
- [ ] Valid/invalid/valid preparation failure leaves every file unchanged.

### Conflict/security

- [ ] Hash mismatch.
- [ ] Size mismatch.
- [ ] Line-count mismatch.
- [ ] External stale modification.
- [ ] Absolute path rejection.
- [ ] Traversal rejection.
- [ ] Symlink escape rejection.

### Text

- [ ] UTF-8.
- [ ] Hindi/Devanagari.
- [ ] Emoji.
- [ ] LF.
- [ ] CRLF.
- [ ] Trailing newline.
- [ ] No trailing newline.

## Verification

- [ ] `cargo test edit`
- [ ] `cargo test edit -- --nocapture`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check`
- [ ] `cargo test`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `git diff --stat`
- [ ] `git diff`
- [ ] `git status`

## Final invariant

> If any validation or preparation step fails, zero files are mutated.