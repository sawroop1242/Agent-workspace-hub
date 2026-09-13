# AWE-002 / #23 — Safe Contextual Replacement

## Agent task
Implement the first production edit primitive using the canonical AWE-001 model.

## Current forensic reality
`ExpectedState` exists, but there is no production edit executor and the existing whole-file MCP write has no stale-state precondition. This issue is the first real mutation implementation.

## Required behavior
- exact old-content matching
- explicit occurrence semantics
- zero-match and unexpected-multiple-match errors
- expected hash validation
- contextual expectation validation
- deterministic conflict result containing expected vs actual state
- in-memory preparation before write
- atomic write through the canonical filesystem boundary
- actual before/after `FileState`
- no mutation on validation/conflict failure

## Security
- reuse canonical workspace path containment
- reject absolute/traversal paths
- do not introduce a third filesystem implementation
- account for the forensic final-component TOCTOU finding; do not claim stronger race guarantees than tests prove

## Edge cases
UTF-8, Unicode/Devanagari/emoji, CRLF/LF, missing trailing newline, empty files, repeated matches, large content within configured limits.

## Do not implement
MCP tools, CLI commands, snapshots, persistent audit, worktrees, agent orchestration, or unrelated policy architecture. Later issues own those surfaces.

## Verification
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Add real temporary-filesystem integration tests. Review `git diff` before completion.
