# AWE-004 — Focused Implementation Plan

## Purpose

Implement multi-operation `filesystem.patch` on top of the canonical edit model and the safe single-operation primitives from AWE-002 and AWE-003.

The core invariant is:

> Prepare and validate the complete patch before mutating any file.

AWE-004 provides pre-commit safety for validation/preparation failures. Crash-safe multi-file rollback remains AWE-006 (#27).

## File Map

### `src/services/edit.rs`

Primary implementation file.

Expected work:

1. Reuse `EditTransaction`, `EditOperation`, `ExpectedState`, `FileState`, `EditStatus`, and `EditError`.
2. Add internal preparation structures such as:
   - `PreparedFileMutation`
   - `PreparedPatch`
3. Extract/reuse pure in-memory transformations for:
   - `Replace`
   - `Insert`
   - `DeleteRange`
4. Add a dispatcher such as `apply_operation_to_content()`.
5. Implement patch preparation:
   - structural validation
   - path validation
   - file loading
   - original state capture
   - expected-state validation
   - ordered in-memory operation application
   - final state calculation
6. Implement patch commit:
   - one final write per affected file
   - no intermediate writes
   - existing filesystem security boundary
7. Implement `EditService::patch()` as orchestration.
8. Perform minimal post-commit verification.

### `src/services/files.rs`

Prefer no changes.

Reuse the existing `FilesService` path and workspace security boundary. Only add a narrowly scoped helper if the existing service cannot support safe preparation/commit without duplication.

### `src/services/mod.rs`

Confirm `edit` is registered. No new module is expected.

## Internal Architecture

```text
EditTransaction
      |
      v
EditService::patch()
      |
      +--> validate transaction
      |
      +--> prepare_patch()
      |      |
      |      +--> resolve/validate paths
      |      +--> read files
      |      +--> capture FileState
      |      +--> validate expected state
      |      +--> apply operations in memory
      |      +--> calculate final FileState
      |
      +--> commit_patch()
      |      |
      |      +--> one final mutation per file
      |
      +--> verify final state
      |
      +--> return canonical result
```

## Same-File Composition

Operations must execute in transaction order against in-memory content:

```text
original -> operation 1 -> state 2 -> operation 2 -> state 3 -> operation 3 -> final
```

Never write intermediate states to disk.

Expected-state checks must apply to the logical state observed by each operation. For example:

```text
H1 --op1--> H2 --op2--> H3
```

An operation expecting `H2` must see `H2`, not the original `H1`.

## Multi-File Safety

Prepare all affected files before the first mutation.

For:

```text
A valid
B invalid
C valid
```

AWE-004 must leave:

```text
A unchanged
B unchanged
C unchanged
```

This is the primary partial-mutation regression test.

## Pure Transformation Rule

Do not duplicate AWE-002/AWE-003 algorithms inside `patch()`.

Prefer a structure similar to:

```rust
fn apply_operation_to_content(
    content: &str,
    operation: &EditOperation,
) -> Result<String, EditError>
```

The helper must only transform in-memory content and return the resulting content. Filesystem writes belong to the commit phase.

## Error Semantics

Use the existing structured taxonomy.

Preparation failures should not be reported as application failures.

Typical mapping:

- invalid path/range/operation -> `ValidationFailed`
- expected state mismatch -> `Conflict`
- write failure -> `ApplyFailed`
- post-write verification failure -> `VerificationFailed`

Do not redesign the error taxonomy unless the current repository requires it.

## Explicit Non-Goals

Do not implement in AWE-004:

- MCP transport
- CLI commands
- TUI changes
- snapshot integration
- provenance system
- audit integration
- policy/capability redesign
- rollback API
- crash recovery
- LLM/agent reasoning
- autonomous conflict resolution
- generic full-file rewrite APIs

These belong to later issues.

## Test Matrix

### Basic

- single replace
- single insert
- single delete
- mixed operations
- empty file

### Same file

- replace -> replace
- replace -> insert
- insert -> replace
- insert -> delete
- delete -> insert
- replace -> delete

### Multi-file

- two-file success
- three-file success
- one final write per affected file
- valid/invalid/valid preparation failure leaves all files unchanged

### Conflicts

- expected hash mismatch
- expected size mismatch
- expected line-count mismatch
- stale external modification
- same-file expected state transition

### Security and text

- absolute path
- traversal
- symlink escape
- UTF-8
- Hindi/Devanagari
- emoji
- LF/CRLF
- trailing newline/no trailing newline

## Verification Commands

```bash
cargo test edit
cargo test edit -- --nocapture
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
git diff --stat
git diff
git status
```

## Completion Gate

AWE-004 is complete only when the implementation demonstrates:

```text
transaction
 -> validate
 -> read
 -> expected-state validation
 -> ordered in-memory composition
 -> prepare all files
 -> commit final states
 -> verify
 -> return result
```

and the critical invariant holds:

> Any validation/preparation failure causes zero filesystem mutation.