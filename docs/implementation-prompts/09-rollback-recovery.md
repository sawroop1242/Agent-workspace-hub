# Prompt 09 — Edit-Level Rollback + Recovery (AWE-010 / TW-006)

## Mission
Implement explicit conflict-aware rollback of one completed edit transaction using canonical edit and snapshot state. This prompt is standalone.

## Required behavior
- Identify the exact EditId; never guess.
- Permit rollback only for an eligible completed edit with exact recoverable prior state.
- Before mutation, verify current filesystem still matches the verified post-edit state.
- Treat newer changes as conflict, never as permission to overwrite.
- Preflight all files in a multi-file rollback before restoring any file.
- Restore exact prior bytes, including newline and Unicode semantics.
- Verify actual restored filesystem state.
- Reuse atomic/recovery primitives.
- Rollback is policy/capability controlled and auditable.
- Repeated rollback and unknown/ineligible IDs have deterministic safe behavior.

## Tests
Cover single/multi-file rollback, newly-created file removal when safe, stale conflict, multi-file conflict with zero mutation, repeated rollback, unknown/failed edit, verification failure, authorization denial, path traversal/symlink escape, and exact bytes.

## Non-goals
No whole-workspace restore, Git reset/revert mechanism, generic undo/redo framework, or separate rollback engine.

## Final report
State eligibility, conflict rule, restoration mechanism, verification, authorization, provenance/audit, tests, and race limitations.
