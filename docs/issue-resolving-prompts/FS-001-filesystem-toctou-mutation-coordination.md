# FS-001 — Filesystem TOCTOU and Mutation Coordination

## Task
Harden canonical filesystem mutation for the multi-agent threat model.

## Forensic baseline
Both existing filesystem implementations have a final-component check-then-use window. Source-file mutations are also not coordinated between agents.

## Requirements
- one canonical filesystem implementation
- safe final-component handling for writes/renames
- preserve traversal/symlink containment
- optional per-path advisory mutation locks where useful
- expected-state conflict enforcement from AWE
- real concurrent process/task tests

## Rule
Do not claim perfect race freedom without a platform-specific proof. Prefer the strongest portable contract and document Linux/Android differences.

## Acceptance
No tested race redirects a write outside the workspace; concurrent edits are detected or serialized according to the documented contract; all planes use the canonical service.
