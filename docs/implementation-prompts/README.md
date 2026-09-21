# AWH Implementation Prompts

This directory is the single canonical home for implementation prompts on the `rust` branch.

## Design rules
- Every prompt is standalone and executable against the current repository.
- No prompt requires another prompt, another prompt's PR, or a prescribed implementation order.
- Each prompt has one primary responsibility; shared concepts are contracts, not competing implementations.
- Reuse existing repository implementations when present; if absent, implement only the minimum local behavior required by the prompt.
- External agents remain development/evaluation infrastructure. AWH runtime remains agent-agnostic.

## Consolidation map
- 01: TW-001 init/runtime identity
- 02: TW-002 + AGENT-001 agent identity/profile/registry/session
- 03: TW-003 + SEC-001 + SEC-002 MCP routing/security
- 04: AWE-001 canonical edit transaction model
- 05: AWE-002..005 plus AWE-004 checklist/plan/verification
- 06: AWE-006..008 edit safety
- 07: AWE-011 + TW-004 edit authorization
- 08: AWE-012 + TW-005 snapshots/provenance
- 09: AWE-010 + TW-006 rollback/recovery
- 10: AWE-013 + TW-007 persistent audit
- 11: AWE-009 + AWE-016 MCP editing/client validation
- 12: AWE-014 CLI editing
- 13: GIT-001 worktree isolation
- 14: FS-001 filesystem coordination/TOCTOU
- 15: ARCH-001 service/store convergence
- 16: AWE-015 + AWE-017 + TW-008 testing/acceptance
- 17: AWE-018 + AWE-019 editing contract/status/roadmap

The old `docs/issue-resolving-prompts/` and `docs/trust-wedge/` collections are intentionally replaced by this single non-overlapping collection.

## Universal completion contract
Every prompt must inspect the current branch, preserve existing behavior unless its scope changes it, fail closed for security-sensitive operations, avoid duplicate services, use real boundaries where required, run applicable verification gates, and report implementation/tests/changed files/limitations. No prompt may claim completion merely because another prompt exists.
