# AWE-004 Coding Checklist

## Before coding
- [ ] Read AWE-001/002/003 prompts and issue bodies.
- [ ] Inspect current `src/services/edit.rs` and `src/services/files.rs`.
- [ ] Search for existing edit executors/callers.
- [ ] Confirm no MCP/CLI editor needs to be preserved yet.

## Implementation
- [ ] Canonical transaction validation.
- [ ] Resolve all paths before writes.
- [ ] Load all affected files first.
- [ ] Capture before states.
- [ ] Validate expected state/context.
- [ ] Apply operations in memory.
- [ ] Reuse AWE-002 replacement helper.
- [ ] Reuse AWE-003 line helpers.
- [ ] Compute after states.
- [ ] Commit only prepared mutations.
- [ ] Verify actual results.
- [ ] Return structured edit result.

## Safety
- [ ] No writes during preparation.
- [ ] No silent stale overwrite.
- [ ] No path-security duplication.
- [ ] No full-file rewrite fallback.
- [ ] No claim of crash-safe rollback.

## Tests
- [ ] Same-file composition.
- [ ] Multi-file patch.
- [ ] Invalid later operation leaves every file unchanged.
- [ ] Stale expected state leaves every file unchanged.
- [ ] Traversal/symlink cases.
- [ ] UTF-8/CRLF/LF/EOF.
- [ ] Real filesystem integration test.

## Gate
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
