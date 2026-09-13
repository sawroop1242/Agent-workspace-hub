# AWE-004 / #25 — Multi-Operation `filesystem.patch`

## Agent task
Implement the canonical patch transaction that composes AWE-002 and AWE-003 operations.

## Forensic reality
`EditTransaction` is currently not constructed in production and no `filesystem.*` MCP tools exist. This issue creates the first real transaction executor.

## Hard invariant
**Prepare and validate every operation and every affected file before mutating any file.**

## Required pipeline
```text
validate request
→ load affected files
→ capture current FileState
→ validate expected state/context
→ apply operations in memory in transaction order
→ compute final FileState
→ commit final content
→ verify actual state
→ return one edit result
```

## Requirements
- same-file operations compose in memory
- multi-file preparation completes before first write
- one stable `EditId`
- canonical path/security boundary
- no duplicate replace/insert/delete algorithms
- deterministic ordering
- structured conflict/validation/apply/verification errors
- one final mutation per affected file where practical
- zero mutation on preparation failure

## Required failure test
A valid; B invalid; C valid → A, B, and C remain unchanged.

## Explicit boundary
Do **not** claim crash-safe rollback here. Semantic rollback and persistent snapshots belong to AWE-006/AWE-010/AWE-012.

## Verification
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Run real temporary-filesystem integration tests, inspect `git diff`, and stop on any failed gate.
