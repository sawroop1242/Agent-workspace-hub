# Prompt 13 — Agent Worktree Isolation (GIT-001)

## Mission
Implement first-class Git worktree isolation bound to AWH agent sessions. This prompt is standalone.

## Required behavior
- Give isolated agent sessions dedicated worktrees when Git-backed.
- Bind worktree ownership explicitly to workspace/session identity.
- Prevent normal AWH paths from mutating another agent's worktree.
- Handle create/list/inspect/remove/recovery states deterministically.
- Never silently reset unrelated branches or indexes.
- Clean up safely on session termination where policy permits.
- Audit worktree lifecycle operations.

## Forensics
Inspect Git helpers, workspace/session identity, filesystem containment, CLI/service layers, and tests. Reuse Git abstractions.

## Tests
Cover simultaneous agents, path isolation, duplicate worktree identity, cleanup/restart, missing worktree, repository errors, and unauthorized cross-worktree access.

## Non-goals
No Git hosting integration, merge engine, agent orchestration, or model execution.

## Final report
Document lifecycle, isolation boundary, cleanup/recovery, tests, and Git/platform limitations.
