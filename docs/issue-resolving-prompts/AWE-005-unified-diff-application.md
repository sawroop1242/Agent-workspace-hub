# AWE-005 / #26 — Unified Diff Application

## Task
Implement unified-diff parsing/application through the canonical EditService.

## Forensic baseline
`ApplyDiff` exists only as a model variant; no parser or executor exists.

## Requirements
- Parse standard file headers and multiple hunks deterministically.
- Reject absolute/traversal paths.
- Validate context against the current file before mutation.
- Support multi-file diffs through AWE-004 preparation semantics.
- Return one edit ID plus before/after states.
- Explicitly reject unsupported binary patches rather than corrupting them as UTF-8.

## Invariants
Malformed diff, path rejection, context mismatch, or stale expected state must produce **zero mutation**.

## Do not duplicate
Use AWE-002 replacement/state semantics and AWE-004 transaction preparation/commit.

## Verification
Run fmt/check/test/clippy, plus real filesystem tests for valid, malformed, stale, multi-hunk, multi-file, and EOF/newline cases.
