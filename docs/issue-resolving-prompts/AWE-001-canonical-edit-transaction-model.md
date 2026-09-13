# AWE-001 / #22 — Canonical Edit Transaction Model

## Agent task
Implement or reconcile the canonical transport-independent edit domain model. The current repository already contains the AWE-001 vocabulary in `src/services/edit.rs`; do not recreate it. Inspect it first and fill only verified gaps.

## Forensic constraints from PR #43
- `EditTransaction` currently has no production callers.
- `ExpectedState.context` must not exist as a decorative field; define its matching semantics.
- File snapshots, provenance, rollback, MCP tools, and capability enforcement are later milestones.

## Required model
- `EditId`
- caller identity hooks: `agent_id`, `session_id`, `workspace_id` where available
- target path
- `EditOperation::{Replace,Insert,DeleteRange,Patch,ApplyDiff}`
- `ExpectedState` with hash/context/size/line expectations as appropriate
- `FileState` with hash/size/line count
- explicit lifecycle status
- structured edit errors
- optional references for snapshot/provenance/audit without coupling the model to storage

## Rules
1. Keep the model transport-independent.
2. Do not implement filesystem mutation here.
3. Preserve existing public semantics unless tests prove they are wrong.
4. Prefer minimal patches.
5. Add focused unit/serialization tests.

## Verification
```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Then inspect `git diff` and stop if any gate fails.

## Done when
AWE-002 and AWE-003 can consume one unambiguous canonical model without duplicating identity/state/error concepts.
