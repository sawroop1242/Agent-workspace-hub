# GIT-001 — Agent Worktree Isolation

## Task
Implement first-class Git worktrees bound to agent sessions.

## Forensic baseline
There are currently zero worktree references in `src/`. Multiple agents share one working tree/index.

## Requirements
- agent → session → workspace → worktree binding
- safe create/list/inspect/remove
- branch allocation
- path containment per worktree
- Git operations target the correct worktree
- explicit merge/handoff/conflict reporting
- audit/provenance includes worktree identity
- cleanup is recoverable

## Dependencies
Wait for AGENT-001, AWE-007, and FS-001. Worktrees built before identity/conflict safety would only organize an unsafe model.

## Tests
Use real Git repositories and run two concurrent agent workflows; prove cross-worktree access is rejected and merge conflicts are surfaced without silent resolution.
