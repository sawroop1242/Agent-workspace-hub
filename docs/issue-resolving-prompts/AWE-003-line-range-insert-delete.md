# AWE-003 / #24 — Line-Range Insert/Delete

## Agent task
Implement deterministic line-oriented editing on the canonical edit service.

## Current forensic reality
Line operations are model-only today. The existing workspace write path can silently overwrite a newer file, so every line operation must carry the AWE-002 expected-state discipline.

## Contract
- use one documented 1-based line convention
- insert before/after a line
- inclusive delete range
- explicit empty-file and EOF semantics
- expected hash/context validation before mutation
- stale-state conflict instead of silent overwrite
- return actual before/after `FileState`
- atomic mutation and zero mutation on invalid input

## Edge cases
Empty file, one-line file, first/last line, delete-all, EOF insertion, missing final newline, LF/CRLF, UTF-8/Devanagari/emoji.

## Architecture
Reuse AWE-001 model and AWE-002 matching/state helpers. Do not duplicate path validation or filesystem containment.

## Do not implement
MCP/CLI/TUI exposure, snapshots, audit persistence, worktrees, or multi-agent orchestration.

## Verification
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Use real temporary filesystem tests and inspect the final diff.
